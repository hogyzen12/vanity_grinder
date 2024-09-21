use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

use clap::Parser;
use num_cpus;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    hash::Hash,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};

/// Command-line arguments structure
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Connection URL to the Solana cluster
    #[arg(short, long, default_value = "http://127.0.0.1:8899")]
    connection_url: String,

    /// Prefix to match at the start of the signature
    #[arg(short, long, default_value = "Clps")]
    prefix: String,

    /// Path to the keypair JSON file
    #[arg(short, long, default_value = "/Users/hogyzen12/.config/solana/iso6UaPwnU9xei3y4ZWEBLMoiFXbJknD9nJ6vKnw6jH.json")]
    keypair_path: PathBuf,

    /// Number of worker threads (default: min(number of CPU cores, 4))
    #[arg(short, long)]
    workers: Option<usize>,
}

/// Message types sent from workers to the main thread
enum WorkerMessage {
    Progress {
        worker_id: char,
        attempts: usize,
    },
    Success {
        worker_id: char,
        signature: String,
        transaction: Transaction,
        iterations: usize,
        time_ms: u128,
    },
    Error {
        worker_id: char,
        error: String,
    },
}

struct SharedState {
    rpc_calls: AtomicUsize,
    blockhash: Mutex<(Hash, Instant)>,
}

fn main() {
    let args = Args::parse();
    let num_cpus = num_cpus::get();
    let num_workers = args.workers.unwrap_or_else(|| std::cmp::min(num_cpus, 4));

    println!("Starting grinding with {} worker(s)...", num_workers);

    let keypair = match read_keypair(&args.keypair_path) {
        Ok(kp) => Arc::new(kp),
        Err(e) => {
            eprintln!("Failed to read keypair: {}", e);
            return;
        }
    };

    println!(
        "Loaded keypair with public key: {}",
        keypair.pubkey()
    );

    let stop_flag = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = crossbeam::channel::unbounded::<WorkerMessage>();
    let rpc_client = Arc::new(RpcClient::new(args.connection_url.clone()));
    let shared_state = Arc::new(SharedState {
        rpc_calls: AtomicUsize::new(0),
        blockhash: Mutex::new((
            get_latest_blockhash(&rpc_client, &AtomicUsize::new(0)),
            Instant::now(),
        )),
    });

    let mut handles = Vec::new();
    for worker_id in ('a'..='z').take(num_workers) {
        let sender_clone = sender.clone();
        let prefix = args.prefix.clone();
        let keypair_clone = Arc::clone(&keypair);
        let stop_flag_clone = Arc::clone(&stop_flag);
        let rpc_client_clone = Arc::clone(&rpc_client);
        let shared_state_clone = Arc::clone(&shared_state);

        let handle = thread::spawn(move || {
            worker_thread(
                worker_id,
                prefix,
                keypair_clone,
                sender_clone,
                stop_flag_clone,
                rpc_client_clone,
                shared_state_clone,
            );
        });

        handles.push(handle);
        println!("Worker '{}' created.", worker_id);
    }

    drop(sender);

    let mut successful_transaction = None;

    for msg in receiver {
        match msg {
            WorkerMessage::Progress { worker_id, attempts } => {
                println!("Worker '{}' has tried {} attempts...", worker_id, attempts);
            }
            WorkerMessage::Success {
                worker_id,
                signature,
                transaction,
                iterations,
                time_ms,
            } => {
                println!(
                    "\n✅ Worker '{}' found a matching signature after {} attempts: {}",
                    worker_id, iterations, signature
                );
                println!("⏰ Time Elapsed: {} ms", time_ms);
                println!("Terminating all workers...");
                stop_flag.store(true, Ordering::SeqCst);
                successful_transaction = Some(transaction);
                break;
            }
            WorkerMessage::Error { worker_id, error } => {
                eprintln!("Worker '{}' encountered an error: {}", worker_id, error);
            }
        }
    }

    for handle in handles {
        let _ = handle.join();
    }

    if let Some(transaction) = successful_transaction {
        println!("Sending the successful transaction...");
        match rpc_client.send_and_confirm_transaction(&transaction) {
            Ok(signature) => println!("Transaction sent successfully. Signature: {}", signature),
            Err(e) => eprintln!("Failed to send transaction: {}", e),
        }
    }

    println!(
        "Total RPC calls made: {}",
        shared_state.rpc_calls.load(Ordering::Relaxed)
    );
    println!("Grinding process completed.");
}

fn read_keypair(path: &PathBuf) -> Result<Keypair, Box<dyn std::error::Error>> {
    let mut file = File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let secret_key: Vec<u8> = serde_json::from_str(&contents)?;
    let keypair = Keypair::from_bytes(&secret_key)?;
    Ok(keypair)
}

fn worker_thread(
    worker_id: char,
    prefix: String,
    keypair: Arc<Keypair>,
    sender: crossbeam::channel::Sender<WorkerMessage>,
    stop_flag: Arc<AtomicBool>,
    rpc_client: Arc<RpcClient>,
    shared_state: Arc<SharedState>,
) {
    let tip_receiver1 = Pubkey::from_str("juLesoSmdTcRtzjCzYzRoHrnF8GhVu6KCV7uxq7nJGp").unwrap();
    let tip_receiver2 = Pubkey::from_str("DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL").unwrap();
    let memo_program_id = Pubkey::from_str("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr").unwrap();

    let base_instructions = vec![
        system_instruction::transfer(&keypair.pubkey(), &tip_receiver1, 100_000),
        system_instruction::transfer(&keypair.pubkey(), &tip_receiver2, 100_000),
    ];

    let prefix_lower = prefix.to_lowercase();
    let start_time = Instant::now();
    let mut attempts: usize = 0;

    while !stop_flag.load(Ordering::Relaxed) {
        attempts += 1;

        let blockhash = get_shared_blockhash(&shared_state, &rpc_client);

        let memo_text = format!("{}-{}", worker_id, attempts);
        let memo_instruction = Instruction {
            program_id: memo_program_id,
            accounts: vec![],
            data: memo_text.into_bytes(),
        };

        let mut instructions = base_instructions.clone();
        instructions.push(memo_instruction);
        let message = solana_sdk::message::Message::new(&instructions, Some(&keypair.pubkey()));
        let mut transaction = Transaction::new_unsigned(message);
        transaction.sign(&[keypair.as_ref()], blockhash);

        let signature = transaction.signatures[0].to_string();

        if is_matching_signature(&signature, &prefix_lower) {
            let elapsed = start_time.elapsed().as_millis();
            let _ = sender.send(WorkerMessage::Success {
                worker_id,
                signature,
                transaction,
                iterations: attempts,
                time_ms: elapsed,
            });
            break;
        }

        if attempts % 1_000_000 == 0 {
            let _ = sender.send(WorkerMessage::Progress {
                worker_id,
                attempts,
            });
        }
    }
}

fn get_shared_blockhash(shared_state: &SharedState, rpc_client: &RpcClient) -> Hash {
    let mut blockhash_data = shared_state.blockhash.lock().unwrap();
    let (blockhash, last_update) = *blockhash_data;

    if last_update.elapsed() > Duration::from_secs(10) {
        let new_blockhash = get_latest_blockhash(rpc_client, &shared_state.rpc_calls);
        *blockhash_data = (new_blockhash, Instant::now());
        new_blockhash
    } else {
        blockhash
    }
}

fn get_latest_blockhash(rpc_client: &RpcClient, rpc_calls: &AtomicUsize) -> Hash {
    rpc_calls.fetch_add(1, Ordering::Relaxed);
    rpc_client.get_latest_blockhash().unwrap()
}

fn is_matching_signature(signature: &str, prefix: &str) -> bool {
    let sig_chars: Vec<char> = signature.to_lowercase().chars().collect();
    let prefix_chars: Vec<char> = prefix.chars().collect();

    sig_chars.iter().zip(prefix_chars.iter()).all(|(s, p)| {
        match p {
            'a'..='z' => s == p,
            '0'..='9' => s.is_numeric() && s == p,
            _ => false,
        }
    })
}