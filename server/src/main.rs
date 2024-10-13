use std::sync::{Arc, Mutex};
use std::io::{prelude::*, ErrorKind};
use std::net::{TcpStream, TcpListener};
use std::sync::atomic::{Ordering, AtomicUsize};
use std::thread::{self, sleep};
use std::time::Duration;
use rand::rngs::OsRng;
use rand::Rng;
use config::{create_auth_response_package, create_empty_package, create_game_package, get_package_type, unpack_auth_request_package, DataType, AUTH_REQUEST_SIZE, AUTH_REQUEST_VERSION, AUTH_RESPONSE_SIZE, PACKET_INFO_SIZE};


static ID_COUNTER: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug)]
enum EventType {
    Write(Arc<Mutex<TcpStream>>, Vec<u8>),
}

#[derive(Debug)]
struct Event {
    pub event_type: EventType,
}

impl Event {
    fn new(event_type: EventType) -> Self {
        Event {
            event_type,
        }
    }
}

struct Client {
    pub token: Option<String>,
    pub id: usize,
    pub address: String,
    pub stream: Arc<Mutex<TcpStream>>,
    pub message_buffer: Vec<String>,
    pub connected: bool,
    pub authenticated: bool,
}

impl Client {
    fn new(stream: Arc<Mutex<TcpStream>>, address: String) -> Self {
        let unique_id = ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        Client {
            token: None,
            id: unique_id,
            address,
            stream,
            message_buffer: Vec::new(),
            connected: true,
            authenticated: false,
        }
    }
}

fn handle_client(client: Arc<Mutex<Client>>, events: Arc<Mutex<Vec<Event>>>) {
    println!("New connection from {}", client.lock().unwrap().address);
    reading_thread(client, events);
}

fn authenticate_client(username: String, password: String) -> String {
    // TODO: Check if arrays username and password have valid .to_vec() results.
    println!("username: '{:?}' password: '{:?}'", username, password);
    generate_session_token(32)
}

fn reading_thread(client: Arc<Mutex<Client>>, events: Arc<Mutex<Vec<Event>>>) {
    thread::spawn(move || {
        loop {
            {
                let stream_mutex =  {
                    let guarded_client = client.lock().unwrap();
                    guarded_client.stream.clone()
                };

                let version: Option<u8>;
                let encoding: Option<u8>;
                let package_type: Option<DataType>;

                let mut info_buffer = [0u8; PACKET_INFO_SIZE];
                {
                    let mut stream = stream_mutex.lock().unwrap();
                    match stream.read(&mut info_buffer) {
                        Ok(0) => {
                            println!("Client {}: closed connection.", client.lock().unwrap().id);
                            return;
                        }
                        Ok(info_bytes_read) => {
                            if info_bytes_read != PACKET_INFO_SIZE {
                                print!("Packet info bytes size: {} expected: {}", info_bytes_read, PACKET_INFO_SIZE);
                                continue;
                            }
                            let (result_version, result_encoding, result_package_type) = get_package_type(info_buffer);
                            version = Some(result_version);
                            encoding = Some(result_encoding);
                            package_type = Some(result_package_type);
                        }
                        Err(e) if e.kind() == ErrorKind::ConnectionReset => {
                            println!("Connection lost to client. {}", e);
                            client.lock().unwrap().connected = false;
                            return;
                        }
                        Err(_e) => {
                            continue;
                        }
                    }
                }
                if !version.is_none() {
                    let unwrapped_version = version.unwrap();
                    let _unwrapped_encoding = encoding.unwrap();
                    let unwrapped_package_type = package_type.unwrap();

                    match unwrapped_package_type {
                        DataType::AuthRequest if unwrapped_version == AUTH_REQUEST_VERSION => {
                            let auth_buffer = {
                                let mut stream = stream_mutex.lock().unwrap();
                                let mut auth_buffer = [0u8; AUTH_REQUEST_SIZE];
                                match stream.read(&mut auth_buffer) {
                                    Ok(auth_bytes_read) if auth_bytes_read == AUTH_REQUEST_SIZE => {
                                        Some(auth_buffer)
                                    }
                                    Ok(auth_bytes_read) => {
                                        println!("Auth request bytes size: {} expected: {}", auth_bytes_read, AUTH_REQUEST_SIZE);
                                        continue;
                                    }
                                    Err(_e) => {
                                        continue;
                                    }
                                }
                            };
                            if let Some(auth_buffer) = auth_buffer {
                                println!("Auth request received!");
                                let (auth_username, auth_password) = unpack_auth_request_package(&auth_buffer);
                                let token = authenticate_client(auth_username, auth_password);
                                let send_data = create_auth_response_package(token);
                                let event = Event::new(EventType::Write(stream_mutex.clone(), send_data.to_vec()));
                                let guarded_events = &mut events.lock().unwrap();
                                guarded_events.push(event);
                                println!("New event! Events: {}", guarded_events.len());
                            }
                        }
                        DataType::Ping => {
                            let send_data = create_empty_package(DataType::Ping);
                            let event = Event::new(EventType::Write(client.lock().unwrap().stream.clone(), send_data.to_vec()));
                            let guarded_events = &mut events.lock().unwrap();
                            guarded_events.push(event);
                        }
                        unexpected_value @ _ => {
                            println!("Unexpected value {:?}", unexpected_value);
                        }
                    }
                }
            }
            sleep(Duration::from_millis(1));
        }
    });
}

fn generate_session_token(length: usize) -> String {
    let charset: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                           abcdefghijklmnopqrstuvwxyz\
                           0123456789\
                           !@#$%^&*()_-+=<>?";
    let token: String = (0..length)
        .map(|_| {
            let idx = OsRng.gen_range(0..charset.len());
            charset[idx] as char
        })
        .collect();

    token
}

fn run_server() -> std::io::Result<()> {
    let events: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
    let clients: Arc<Mutex<Vec<Arc<Mutex<Client>>>>> = Arc::new(Mutex::new(Vec::new()));

    let cloned_events = Arc::clone(&events);
    let _cloned_clients = Arc::clone(&clients);

    thread::spawn(move || {
        loop {
            {
                let guarded_events = &mut cloned_events.lock().unwrap();
                if guarded_events.is_empty() {
                    continue;
                }
                for event in guarded_events.drain(..) {
                    match &event.event_type {
                        EventType::Write(stream, message) => {
                            let guarded_stream = &mut stream.lock().unwrap();
                            let _ = guarded_stream.write(message);
                        }
                    }
                }
            }
            sleep(Duration::from_millis(1));
        }
    });
    match TcpListener::bind("127.0.0.1:8080") {
        Ok(listener) => {
            println!("Server is running!");
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let address = stream.peer_addr().unwrap().to_string();
                        if address.starts_with("127.0.0.1") {
                            let _ = stream.set_nonblocking(true);
                            let mutex_stream = Arc::new(Mutex::new(stream));
                            let client = Arc::new(Mutex::new(Client::new(mutex_stream, address)));
                            let guarded_clients = &mut clients.lock().unwrap();
                            guarded_clients.push(client.clone());
                            handle_client(client.clone(), events.clone());
                        } else {
                            println!("Outside connection! {}", stream.peer_addr().unwrap().to_string())
                        }
                    },
                    Err(e) => { println!("Connection failed! {}", e)}
                }
            }
        }
        Err(e) => {
            println!("Failed to bind! {}", e);
        }
    }
    Ok(())
}

fn main() {
    let _ = run_server();
}
