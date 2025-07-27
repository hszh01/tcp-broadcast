use std::collections::HashMap;
use std::env;
use std::io;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::{TcpListener, TcpStream};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

use tokio_stream::wrappers::{LinesStream, TcpListenerStream};
use tokio_stream::{StreamExt, StreamMap};

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    // Choose port: default 8888 or first CLI arg.
    let port: u16 = env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(8888);

    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(&addr).await?;
    println!("listening on port {port}");

    // Stream of incoming connections
    let mut incoming = TcpListenerStream::new(listener);

    // Map of client_id -> write half
    let mut writers: HashMap<u16, BufWriter<OwnedWriteHalf>> = HashMap::new();

    // Map of client_id -> stream of input lines
    let mut inputs: StreamMap<u16, LinesStream<BufReader<OwnedReadHalf>>> = StreamMap::new();

    loop {
        tokio::select! {
            // Accept new clients
            maybe_conn = incoming.next() => {
                match maybe_conn {
                    Some(Ok(stream)) => {
                        if let Err(e) = handle_new_client(stream, &mut writers, &mut inputs).await {
                            eprintln!("error on accept: {e}");
                        }
                    }
                    Some(Err(e)) => {
                        eprintln!("accept error: {e}");
                    }
                    None => {
                        break;
                    }
                }
            }

            // Any line from any client
            maybe_item = inputs.next(), if !inputs.is_empty() => {
                match maybe_item {
                    Some((client_id, Ok(line))) => {

                        println!("message {client_id} {line}");

                        // Broadcast to all other clients
                        let mut dead: Vec<u16> = Vec::new();
                        let msg = format!("MESSAGE:{client_id} {line}\n");
                        for (&other_id, w) in writers.iter_mut() {
                            if other_id == client_id { continue; }
                            if let Err(e) = w.write_all(msg.as_bytes()).await {
                                eprintln!("write error to {other_id}: {e}");
                                dead.push(other_id);
                                continue;
                            }
                            if let Err(e) = w.flush().await {
                                eprintln!("flush error to {other_id}: {e}");
                                dead.push(other_id);
                            }
                        }
                        // Remove any failed writers
                        for id in dead {
                            writers.remove(&id);
                            inputs.remove(&id);
                        }

                        // ACK to sender
                        if let Some(w) = writers.get_mut(&client_id) {
                            if let Err(e) = w.write_all(b"ACK:MESSAGE\n").await {
                                eprintln!("ack write error to {client_id}: {e}");
                                writers.remove(&client_id);
                                inputs.remove(&client_id);
                            } else if let Err(e) = w.flush().await {
                                eprintln!("ack flush error to {client_id}: {e}");
                                writers.remove(&client_id);
                                inputs.remove(&client_id);
                            }
                        }
                    }
                    Some((client_id, Err(e))) => {
                        eprintln!("read error from {client_id}: {e}");
                        writers.remove(&client_id);
                        inputs.remove(&client_id);
                    }
                    None => {
                        // No more input streams (all clients gone) — keep accepting
                        // (the accept branch above will continue to fire).
                    }
                }
            }
        }
    }

    Ok(())
}

async fn handle_new_client(
    stream: TcpStream,
    writers: &mut HashMap<u16, BufWriter<OwnedWriteHalf>>,
    inputs: &mut StreamMap<u16, LinesStream<BufReader<OwnedReadHalf>>>,
) -> io::Result<()> {
    let peer = stream.peer_addr()?;
    let client_id: u16 = peer.port(); // use peer port as CLIENT_ID to match examples

    println!("connected {} {}", peer.ip(), client_id);

    let (read_half, write_half) = stream.into_split();

    // Prepare the reader as a stream of lines
    let reader = BufReader::new(read_half);
    let lines = reader.lines();
    let lines_stream = LinesStream::new(lines);

    // Prepare writer
    let mut writer = BufWriter::new(write_half);

    writer.write_all(format!("LOGIN:{client_id}\n").as_bytes()).await?;
    writer.flush().await?;

    writers.insert(client_id, writer);
    inputs.insert(client_id, lines_stream);

    Ok(())
}