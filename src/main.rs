use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer, BaseConsumer};
use rdkafka::message::Message;
use rdkafka::producer::{FutureProducer, FutureRecord};
use std::env;
use std::ffi::c_float;
use std::process::Command;
use std::time::{Duration, Instant};
use std::thread;
use std::str;
use tokio::time;
use serde::{Serialize, Deserialize};
use serde_json;

const KAFKA_BROKER: &str = "localhost:9092";
const PRODUCER_TOPIC: &str = "usages";
const CONSUMER_TOPIC: &str = "updates";
const INTERVAL: u64 = 10000;

#[derive(Debug, Serialize, Deserialize)]
struct Stats {
    #[serde(rename = "BlockIO")]
    block_io: String,
    #[serde(rename = "CPUPerc")]
    cpu_perc: String,
    #[serde(rename = "Container")]
    container: String,
    #[serde(rename = "ID")]
    id: String,
    #[serde(rename = "MemPerc")]
    mem_perc: String,
    #[serde(rename = "MemUsage")]
    mem_usage: String,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "NetIO")]
    net_io: String,
    #[serde(rename = "PIDs")]
    pids: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct HostConfig {
    #[serde(rename = "NanoCpus")]
    nano_cpus: i64,
    #[serde(rename = "Memory")]
    memory: i64
}

#[derive(Debug, Serialize, Deserialize)]
struct Inspect {
    #[serde(rename = "HostConfig")]
    host_config: HostConfig,
}

#[derive(Debug, Serialize, Deserialize)]
struct Usage {
    cpu_percentage: f32,
    memory_percentage: f32,
}

#[derive(Debug, Serialize, Deserialize)]
struct Update {
    cpu_update: i8,         // can be -1, 0, 1
    memory_update: i8,
}

fn create_producer() -> FutureProducer {
    let producer: FutureProducer = ClientConfig::new()
        .set("bootstrap.servers", KAFKA_BROKER)
        .set("queue.buffering.max.ms", "500".to_string())
        .set("message.timeout.ms", "10000")
        .set("auto.offset.reset", "earliest")
        .create()
        .expect("Producer creation failed");

    producer
}

fn get_json_stats(docker_stats: &String) -> serde_json::Result<Stats> {
    let stats = Command::new("sh")
        .arg("-c")
        .arg(docker_stats)
        .output()
        .expect("failed to execute process");

    let stats_string: String = String::from_utf8_lossy(&stats.stdout).to_string();

    let stats_json: Stats = serde_json::from_str(&stats_string).unwrap();
    Ok(stats_json)
}

fn get_json_inspect(docker_inspect: &String) -> serde_json::Result<Inspect> {
    let inspect = Command::new("sh")
        .arg("-c")
        .arg(docker_inspect)
        .output()
        .expect("failed to execute process");

    let inspect_string: String = String::from_utf8_lossy(&inspect.stdout).to_string();

    let mut inspect_jsons: Vec<Inspect> = serde_json::from_str(&inspect_string).unwrap();
    let inspect_json: Inspect = inspect_jsons.pop().unwrap();
    Ok(inspect_json)
}

async fn send_usages(producer: &FutureProducer, docker_stats: &String, docker_inspect: &String) {
    let docker_container_id: String = env::var("DOCKER_CONTAINER_ID").unwrap();
    
    let stats: Stats = get_json_stats(&docker_stats).unwrap();
    let absolute_cpu_prercentage: f32 = ((stats.cpu_perc).replace("%", "")).parse().unwrap();
    let memory_percentage: f32 = ((stats.mem_perc).replace("%", "")).parse().unwrap();

    let inspect: Inspect = get_json_inspect(&docker_inspect).unwrap();
    let total_cpu: f32 = (inspect.host_config.nano_cpus as f32) / (1000000000 as f32);
    let cpu_percentage: f32 = (absolute_cpu_prercentage / total_cpu);

    let usage = serde_json::json!({
        "cpu_percentage": cpu_percentage,
        "memory_percentage": memory_percentage,
    });

    let usage_message: String = usage.to_string();

    let result: Result<(i32, i64), (rdkafka::error::KafkaError, rdkafka::message::OwnedMessage)> = producer.send(
        FutureRecord::to(PRODUCER_TOPIC).key(&docker_container_id).payload(&usage_message),
        Duration::from_secs(0)
    ).await;
}

#[tokio::main]
async fn main() {
    let docker_container_id: String = env::var("DOCKER_CONTAINER_ID").unwrap();
    let docker_stats: String = format!(
        "docker stats --format json --no-stream {}",
        docker_container_id
    );
    let docker_inspect: String = format!(
        "docker inspect --format json {}",
        docker_container_id
    );
    let consumer_inspect: String = docker_inspect.clone();

    let producer: FutureProducer = create_producer();
    // let consumer = create_consumer();
    let mut consumer: BaseConsumer = ClientConfig::new()
        .set("bootstrap.servers", KAFKA_BROKER)
        .set("group.id", "krosswalk")
        .create()
        .expect("Consumer creation failed");
    consumer.subscribe(&[CONSUMER_TOPIC]).expect("Subscription failed");

    let handle = thread::spawn(move || loop {
            for msg_result in consumer.iter() {
                let msg = msg_result.unwrap();
                let value =  str::from_utf8(msg.payload().unwrap()).unwrap();
                let update_json: Update = serde_json::from_str(&value).unwrap();

                if (update_json.cpu_update != 0 || update_json.memory_update != 0) {
                    println!("inside");
                    let inspect: Inspect = get_json_inspect(&consumer_inspect).unwrap();
                    let total_cpus: f32 = (inspect.host_config.nano_cpus as f32) / (1000000000 as f32);
                    let total_memory: i64 = (inspect.host_config.memory) / (1024 * 1024);

                    let mut updated_cpu: f32 = total_cpus;
                    let mut updated_memory: i64 = total_memory;

                    if (update_json.cpu_update != 0) {
                        updated_cpu = if update_json.cpu_update >= 1 { total_cpus + 0.5 } else { total_cpus - 0.5 };
                    }
                    if (update_json.memory_update != 0) {
                        updated_memory = if update_json.memory_update >= 1 { total_memory + 512 } else { total_memory - 512 };
                    }

                    Command::new("sh")
                        .arg("-c")
                        .arg(format!("docker update --cpus=\"{}\" --memory=\"{}m\" {}", updated_cpu, updated_memory, docker_container_id))
                        .output()
                        .expect("failed to execute process");
                }
            }
        }
    );

    // Wait for the consumer thread to finish (e.g., by pressing Ctrl+C).
    handle.join().expect("Consumer thread panicked");

    let spawner = tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_millis(INTERVAL));
        loop {
            send_usages(&producer, &docker_stats, &docker_inspect).await;
            interval.tick().await;
        }
    });

    spawner.await;
}
