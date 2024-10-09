use std::io::prelude::*;
use std::net::{TcpStream, TcpListener};
use std::env;
use std::sync::atomic::{Ordering, AtomicUsize};

static ID_COUNTER: AtomicUsize = AtomicUsize::new(1);

struct Client {
    id: usize,
    address: String,
    stream: TcpStream,
}


impl Client {
    fn new(stream: TcpStream, address: String) -> Self {
        let unique_id = ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        Client {
            id: unique_id,
            address,
            stream,
        }
    }
}

fn handle_client(client: Client) {
    let mut stream = client.stream;
    println!("New connection from {}", client.address);
    let _ = stream.write("Testing!".as_bytes());
    let _ = stream.flush();

    let mut buffer = [0; 128];

    match stream.read(&mut buffer) {
        Ok(bytes_read) => {
            println!("Client {}: {}", client.id, String::from_utf8_lossy(&buffer[..bytes_read]));
        }
        Err(e) => {
            println!("Failed to read from stream! {}", e);
        }
    }
}

fn run_server() -> std::io::Result<()>{
    let listener = TcpListener::bind("127.0.0.1:8080")?;

    println!("Server is running!");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let address = stream.peer_addr().unwrap().to_string();
                if address.starts_with("127.0.0.1") {
                    let new_client = Client::new(stream, address);
                    handle_client(new_client);
                } else{
                    println!("Outside connection! {}", stream.peer_addr().unwrap().to_string())
                }
            },
            Err(e) => { println!("Connection failed! {}", e)}
        }
    }
    Ok(())
}

fn client() -> std::io::Result<()> {
    if let Ok(mut stream) = TcpStream::connect("127.0.0.1:8080") {
        println!("Connected!");
        let _ = stream.write("Hello!".as_bytes());
        let _ = stream.flush();

        let mut buffer = [0; 128];

        let bytes_read = stream.read(&mut buffer)?;
        println!("Server message: {}", String::from_utf8_lossy(&buffer[..bytes_read]));
    } else {
        println!("Connection failed!");
    }
    Ok(())
}

fn main() {
    let mut args = env::args();
    if let Some(argument) = args.nth(1) {
        match argument.as_str() {
            "server" => {
                let _ = run_server();
            }
            "client" => {
                let _ = client();
            }
            _ => {
                println!("Did not understand arguments! Try 'server' or 'client'");
            }
        }
    }
}
