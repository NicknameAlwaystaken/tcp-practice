use std::sync::{Arc, Mutex};
use std::io::{prelude::*, ErrorKind};
use std::net::{TcpStream, TcpListener};
use std::sync::atomic::{Ordering, AtomicUsize};
use std::thread::{self, sleep};
use std::time::Duration;
use rand::rngs::OsRng;
use rand::Rng;


static ID_COUNTER: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug)]
enum EventType {
    Write(usize, String),
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
    pub stream: TcpStream,
    pub message_buffer: Vec<String>,
    pub connected: bool,
    pub authenticated: bool,
}

impl Client {
    fn new(stream: TcpStream, address: String) -> Self {
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

fn authenticate_client(client: Arc<Mutex<Client>>, message: String) -> Option<String> {
    let guarded_client = &mut client.lock().unwrap();
    let token = generate_session_token(32);
    guarded_client.token = Some(token.clone());
    guarded_client.authenticated = true;
    Some(token)
}

fn reading_thread(client: Arc<Mutex<Client>>, events: Arc<Mutex<Vec<Event>>>) {
    thread::spawn(move || {
        loop {
            {
                let mut stream = {
                    let guarded_client = client.lock().unwrap();
                    guarded_client.stream.try_clone().unwrap()
                };

                let mut buffer = [0; 128];
                match stream.read(&mut buffer) {
                    Ok(0) => {
                        println!("Client {}: closed connection.", client.lock().unwrap().id);
                        return;
                    }
                    Ok(bytes_read) => {
                        let message = String::from_utf8_lossy(&buffer[..bytes_read]);
                        if !client.lock().unwrap().authenticated {
                            if message.starts_with("HELLO") {
                                println!("{}", message.to_string());
                                if let Some(token) = authenticate_client(client.clone(), message.to_string()) {
                                    let event = Event::new(EventType::Write(client.lock().unwrap().id, "TOKEN ".to_string() + &token));
                                    let guarded_events = &mut events.lock().unwrap();
                                    guarded_events.push(event);
                                }
                            }
                        } else {
                            println!("Client {}: {}", client.lock().unwrap().id, message);
                            if message == "PING" {
                                let event = Event::new(EventType::Write(client.lock().unwrap().id, "PONG".to_string()));
                                let guarded_events = &mut events.lock().unwrap();
                                guarded_events.push(event);
                            }
                        }
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
    let cloned_clients = Arc::clone(&clients);

    thread::spawn(move || {
        loop {
            {
                let guarded_events = &mut cloned_events.lock().unwrap();
                if guarded_events.is_empty() {
                    continue;
                }
                for event in guarded_events.drain(..) {
                    match &event.event_type {
                        EventType::Write(client_id, message) => {
                            let guarded_clients = cloned_clients.lock().unwrap();
                            for client in guarded_clients.iter() {
                                let guarded_client = &mut client.lock().unwrap();
                                if guarded_client.id == *client_id {
                                    let _ = guarded_client.stream.write(message.as_bytes());
                                }
                            }
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
                            let client = Arc::new(Mutex::new(Client::new(stream, address)));
                            let guarded_clients = &mut clients.lock().unwrap();
                            guarded_clients.push(client.clone());
                            handle_client(client.clone(), events.clone());
                        } else{
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
