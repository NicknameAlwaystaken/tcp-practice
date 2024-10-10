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
    pub stream: TcpStream,
    pub message_buffer: Vec<String>,
    pub connected: bool,
    pub ping: Option<Ping>,
}

impl Client {
    fn new(stream: TcpStream) -> Self {
        Client {
            stream,
            message_buffer: Vec::new(),
            connected: true,
            ping: None,
        }
    }
}

fn reading_thread(client: Arc<Mutex<Client>>) {
    thread::spawn(move || {
        loop {
            {
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
                        println!("Connection lost.");
                        client.lock().unwrap().connected = false;
                        return;
                    }
                    Err(_e) => {
                        continue;
                    }
                }
            }
            sleep(Duration::from_millis(100));
        }
    });
}

fn ping_server(client: Arc<Mutex<Client>>) {
    thread::spawn(move || {
        loop {
            {
                let mut guarded_client = client.lock().unwrap();
                if !guarded_client.connected {
                    return;
                }
                let message_buffer = &mut guarded_client.message_buffer;
                message_buffer.push("PING".to_string());
                guarded_client.ping = Some(Ping::new(Instant::now()));
            }
            sleep(Duration::from_secs(1));
        }
    });
}

fn client() -> std::io::Result<()> {
    if let Ok(new_stream) = TcpStream::connect("127.0.0.1:8080") {
        let _ = new_stream.set_nonblocking(true);
        let client = Arc::new(Mutex::new(Client::new(new_stream)));
        {
            let mut guarded_client = client.lock().unwrap();
            let stream = &mut guarded_client.stream;
            let _ = stream.write("Hello!".as_bytes());
            let _ = stream.flush();
        }

        reading_thread(client.clone());
        sending_thread(client.clone());
        ping_server(client.clone());

        println!("Connected to server.");

        loop {
            {
                let guarded_client = client.lock().unwrap();
                if !guarded_client.connected {
                    println!("Disconnected from server.");
                    break;
                }
            }
            sleep(Duration::from_millis(100));
        }
    } else {
        println!("Connection failed!");
    }
    Ok(())
}

fn sending_thread(client: Arc<Mutex<Client>>) {
    thread::spawn(move || {
        loop {
            {
                // create block to isolate checking if client is still connected
                {
                    let guarded_client = client.lock().unwrap();
                    if !guarded_client.connected {
                        return;
                    }
                }
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
            sleep(Duration::from_millis(100));
        }
    });
}

fn main() {
    let _ = client();
}
