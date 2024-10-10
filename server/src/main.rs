use std::sync::{Arc, Mutex};
use std::io::{prelude::*, ErrorKind};
use std::net::{TcpStream, TcpListener};
use std::sync::atomic::{Ordering, AtomicUsize};
use std::thread::{self, sleep};
use std::time::Duration;

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
    pub id: usize,
    pub address: String,
    pub stream: TcpStream,
    pub message_buffer: Vec<String>,
    pub connected: bool,
}

impl Client {
    fn new(stream: TcpStream, address: String) -> Self {
        let unique_id = ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        Client {
            id: unique_id,
            address,
            stream,
            message_buffer: Vec::new(),
            connected: true,
        }
    }
}

fn handle_client(client: Arc<Mutex<Client>>, events: Arc<Mutex<Vec<Event>>>) {
    println!("New connection from {}", client.lock().unwrap().address);
    reading_thread(client, events);
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
                        println!("Client {}: {}", client.lock().unwrap().id, message);

                        if message == "PING" {
                            let event = Event::new(EventType::Write(client.lock().unwrap().id, "PONG".to_string()));
                            let guarded_events = &mut events.lock().unwrap();
                            guarded_events.push(event);
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

fn run_server() -> std::io::Result<()> {
    let events: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
    let clients: Arc<Mutex<Vec<Arc<Mutex<Client>>>>> = Arc::new(Mutex::new(Vec::new()));

    let cloned_events = Arc::clone(&events);
    let cloned_clients = Arc::clone(&clients);

    thread::spawn(move || {
        loop {
            {
                let guarded_events = &mut cloned_events.lock().unwrap();
                if !guarded_events.is_empty() {
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
