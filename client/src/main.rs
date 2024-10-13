use std::io::{prelude::*, ErrorKind};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread::{self, sleep};
use std::time::{Duration, Instant};
use config::{create_auth_request_package, create_empty_package, get_package_type, unpack_auth_response_package, DataType, AUTH_RESPONSE_SIZE, AUTH_RESPONSE_VERSION, PACKET_INFO_SIZE};

struct Ping {
    pub sent_at: Instant,
    pub received_at: Option<Instant>,

}

impl Ping {
    fn new(sent_at: Instant) -> Self {
        Ping {
            sent_at,
            received_at: None,
        }
    }

    fn receive(&mut self) {
        if self.received_at.is_none() {
            self.received_at = Some(Instant::now());
        }
    }

    fn get_duration(&self) -> Option<Duration> {
        if let Some(received_at) = self.received_at {
            Some(received_at.duration_since(self.sent_at))
        } else {
            None
        }
    }
}

struct Client {
    pub authenticated: bool,
    pub stream: TcpStream,
    pub message_buffer: Vec<Vec<u8>>,
    pub connected: bool,
    pub retrying: bool,
    pub ping: Ping,
}

impl Client {
    fn new(stream: TcpStream) -> Self {
        Client {
            authenticated: false,
            stream,
            message_buffer: Vec::new(),
            connected: false,
            retrying: false,
            ping: Ping::new(Instant::now()),
        }
    }
}

fn reading_thread(client: Arc<Mutex<Client>>) {
    thread::spawn(move || {
        'outer: loop {
            {
                let is_connected = {
                    client.lock().unwrap().connected
                };
                let is_disconnected_and_not_retrying = {
                    !is_connected && client.lock().unwrap().retrying
                };
                if is_disconnected_and_not_retrying {
                    println!("Closing stream reading");
                    break 'outer;
                }
                let is_authenticated = {
                    let guarded_client = client.lock().unwrap();
                    guarded_client.authenticated
                };
                if is_connected && is_authenticated {
                    let mut stream = {
                        let guarded_client = client.lock().unwrap();
                        guarded_client.stream.try_clone().unwrap()
                    };

                    let mut buffer = [0; PACKET_INFO_SIZE];
                    match stream.read(&mut buffer) {
                        Ok(bytes_read) => {
                            if bytes_read != PACKET_INFO_SIZE {
                                print!("Read bytes size: {} expected: {}", bytes_read, PACKET_INFO_SIZE);
                                continue;
                            }
                            let (_version, _encoding, package_type) = get_package_type(buffer);
                            match package_type {
                                DataType::Ping => {
                                    let guarded_client = &mut client.lock().unwrap();
                                    let ping = &mut guarded_client.ping;
                                    ping.receive();
                                    println!("Ping: {:?}", ping.get_duration().unwrap());
                                }
                                DataType::Disconnect => {
                                    println!("Disconnecting...");
                                }
                                unexpected_value @ _ => {
                                    println!("Unexpected data type {:?}", unexpected_value);
                                }
                            }
                        }
                        Err(e) if e.kind() == ErrorKind::ConnectionReset => {
                            if client.lock().unwrap().retrying == false {
                                let guarded_client = &mut client.lock().unwrap();
                                println!("Connection lost. Retrying...");
                                guarded_client.connected = false;
                                guarded_client.retrying = true;
                            }
                        }
                        Err(_e) => {
                            continue;
                        }
                    }
                }
            }
            sleep(Duration::from_millis(1));
        }
    });
}

fn sending_thread(client: Arc<Mutex<Client>>) {
    thread::spawn(move || {
        'outer: loop {
            {
                let is_connected = {
                    client.lock().unwrap().connected
                };
                let is_disconnected_and_not_retrying = {
                    !is_connected && !client.lock().unwrap().retrying
                };
                if is_disconnected_and_not_retrying {
                    println!("Closing stream reading");
                    break 'outer;
                }
                let is_authenticated = {
                    client.lock().unwrap().authenticated
                };
                if is_connected  && is_authenticated {
                    let message_to_send = {
                        let mut guarded_client = client.lock().unwrap();

                        if guarded_client.message_buffer.is_empty() {
                            continue;
                        }

                        guarded_client.message_buffer.drain(..).collect::<Vec<_>>()
                    };
                    let mut guarded_client = client.lock().unwrap();
                    let stream = &mut guarded_client.stream;

                    for message in message_to_send {
                        let _ = stream.write(&message);
                        let _ = stream.flush();
                    }
                }
            }
            sleep(Duration::from_millis(1));
        }
    });
}

fn ping_server(client: Arc<Mutex<Client>>) {
    thread::spawn(move || {
        'outer: loop {
            {
                let is_connected = {
                    client.lock().unwrap().connected
                };
                let is_disconnected_and_not_retrying = {
                    !is_connected && !client.lock().unwrap().retrying
                };
                if is_disconnected_and_not_retrying {
                    println!("Closing stream reading");
                    break 'outer;
                }
                let is_authenticated = {
                    client.lock().unwrap().authenticated
                };
                if is_connected && is_authenticated {
                    let guarded_client = &mut client.lock().unwrap();
                    let send_data = create_empty_package(DataType::Ping);
                    guarded_client.message_buffer.push(send_data.to_vec());
                    if guarded_client.ping.received_at != None {
                        guarded_client.ping = Ping::new(Instant::now());
                    }
                }
            }
            sleep(Duration::from_secs(1));
        }
    });
}

fn initialize_connection(client: Arc<Mutex<Client>>) -> bool{
    let mut stream = {
        let guarded_client = client.lock().unwrap();
        guarded_client.stream.try_clone().unwrap()
    };
    let send_data = create_auth_request_package("username".to_string(), "password".to_string());
    println!("{:?}", send_data);
    let _ = stream.write(&send_data);
    let _ = stream.flush();

    let mut buffer = [0u8; PACKET_INFO_SIZE];
    println!("Waiting for authentication.");
    let _ = stream.set_nonblocking(false);
    loop {
        match stream.read(&mut buffer) {
            Ok(bytes_read) => {
                if bytes_read != PACKET_INFO_SIZE {
                    print!("Read bytes size: {} expected: {}", bytes_read, PACKET_INFO_SIZE);
                    continue;
                }
                let (version, _encoding, package_type) = get_package_type(buffer);
                match package_type {
                    DataType::AuthResponse if version == AUTH_RESPONSE_VERSION => {
                        let mut response_buffer = [0u8; AUTH_RESPONSE_SIZE];
                        match stream.read(&mut response_buffer) {
                            Ok(response_bytes_read) => {
                                if response_bytes_read != AUTH_RESPONSE_SIZE {
                                    print!("Read bytes size: {} expected: {}", response_bytes_read, AUTH_RESPONSE_SIZE);
                                    continue;
                                }
                                let token = unpack_auth_response_package(&response_buffer);
                                if token == "0" {
                                    println!("Authentication failed.");
                                    return false;
                                } else {
                                    let guarded_client = &mut client.lock().unwrap();
                                    guarded_client.authenticated = true;
                                    guarded_client.connected = true;
                                    println!("Authenticated!");
                                    return true;
                                }

                            }
                            unexpected_value @ _ => {
                                println!("Data type unknown {:?}", unexpected_value);
                            }
                        }
                    }
                    unexpected_value @ _ => {
                        println!("Data type unknown {:?}", unexpected_value);
                    }
                }
            }
            Err(_e) => {
                println!("Could not authenticate");
            }
        }
        println!("Retrying in 2 seconds...");
        sleep(Duration::from_secs(2));
    }
}

fn client() -> std::io::Result<()> {
    if let Ok(new_stream) = TcpStream::connect("127.0.0.1:8080") {
        println!("Connected to server.");
        let client = Arc::new(Mutex::new(Client::new(new_stream)));

        let connection_initialized = {
            initialize_connection(client.clone())
        };
        if connection_initialized {
            {
                let guarded_client = &mut client.lock().unwrap();
                let _ = guarded_client.stream.set_nonblocking(true);
            }
            println!("Finished initialization");
            reading_thread(client.clone());
            ping_server(client.clone());
            sending_thread(client.clone());

            let cloned_client = client.clone();
            loop {
                {
                    if !cloned_client.lock().unwrap().connected {
                        println!("Reconnecting...");
                        if let Ok(new_stream) = TcpStream::connect("127.0.0.1:8080") {
                            {
                                let guarded_client = &mut cloned_client.lock().unwrap();
                                guarded_client.stream = new_stream;
                                guarded_client.connected = true;
                                guarded_client.retrying = false;
                            }
                            println!("Reconnected!");
                            continue;
                        }
                        else {
                            println!("Failed to reconnect.");
                        }
                    }
                    sleep(Duration::from_secs(2))
                }
                sleep(Duration::from_millis(500));
            }
        }
    } else {
        println!("Connection failed!");
    }
    Ok(())
}

fn main() {
    let _ = client();
}
