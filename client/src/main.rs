use std::io::{prelude::*, ErrorKind};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread::{self, sleep};
use std::time::{Duration, Instant};

struct Ping {
    pub sent_at: Instant,
    pub received_at: Option<Instant>,

}

impl Ping {
    fn new(sent_at: Instant) -> Self{
        Ping {
            sent_at,
            received_at: None,
        }
    }

    fn receive(&mut self) {
        self.received_at = Some(Instant::now());
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
    pub token: Option<String>,
    pub stream: TcpStream,
    pub message_buffer: Vec<String>,
    pub connected: bool,
    pub retrying: bool,
    pub ping: Option<Ping>,
}

impl Client {
    fn new(stream: TcpStream) -> Self {
        Client {
            token: None,
            stream,
            message_buffer: Vec::new(),
            connected: false,
            retrying: false,
            ping: None,
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
                    guarded_client.token.is_some()
                };
                if is_connected && is_authenticated {
                    let mut stream = {
                        let guarded_client = client.lock().unwrap();
                        guarded_client.stream.try_clone().unwrap()
                    };

                    let mut buffer = [0; 128];
                    match stream.read(&mut buffer) {
                        Ok(bytes_read) => {
                            let server_message = String::from_utf8_lossy(&buffer[..bytes_read]);
                            if server_message == "PONG" {
                                if let Some(ref mut ping) = client.lock().unwrap().ping {
                                    ping.receive();
                                    println!("Ping to server: {:?}", ping.get_duration().unwrap());
                                }
                            } else {
                                println!("Server message: {}", server_message);
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
                    client.lock().unwrap().token.is_some()
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
                        let _ = stream.write(message.as_bytes());
                        let _ = stream.flush();
                    }
                }
            }
            sleep(Duration::from_millis(100));
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
                    client.lock().unwrap().token.is_some()
                };
                if is_connected && is_authenticated {
                    let guarded_client = &mut client.lock().unwrap();
                    let message_buffer = &mut guarded_client.message_buffer;
                    message_buffer.push("PING".to_string());
                    guarded_client.ping = Some(Ping::new(Instant::now()));
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
    let _ = stream.write(("HELLO".to_string()).as_bytes());
    let _ = stream.flush();

    let mut buffer = [0; 128];
    println!("Waiting for authentication.");
    let _ = stream.set_nonblocking(false);
    loop {
        match stream.read(&mut buffer) {
            Ok(bytes_read) => {
                let server_message = String::from_utf8_lossy(&buffer[..bytes_read]);
                if server_message.starts_with("TOKEN") {
                    let token = server_message.split(" ").nth(1).unwrap().to_string();
                    let guarded_client = &mut client.lock().unwrap();
                    guarded_client.token = Some(token.clone());
                    guarded_client.connected = true;
                    println!("Authenticated with token: `{}`", &token);
                    return true;
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
            {
                reading_thread(client.clone());
                ping_server(client.clone());
                sending_thread(client.clone());
            }

            let cloned_client = client.clone();
            loop {
                {
                    if !cloned_client.lock().unwrap().connected {
                        println!("Reconnecting...");
                        if let Ok(new_stream) = TcpStream::connect("127.0.0.1:8080") {
                            let mut stream = {
                                let guarded_client = &mut cloned_client.lock().unwrap();
                                guarded_client.stream = new_stream;
                                guarded_client.connected = true;
                                guarded_client.retrying = false;
                                guarded_client.stream.try_clone().unwrap()
                            };
                            let _ = stream.write("I am back!".as_bytes());
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
