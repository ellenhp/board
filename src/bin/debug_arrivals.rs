use board::{fetch_arrivals, format_time, recalculate_and_filter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("Usage: debug_arrivals <stop_name> <stop_id> [stop_id...]");
        std::process::exit(1);
    }
    let stop_name = &args[0];
    let stop_ids: Vec<&str> = args[1..].iter().map(|s| s.as_str()).collect();

    println!("Fetching arrivals for: {}", stop_name);
    println!("Stop IDs: {:?}\n", stop_ids);

    let mut all_arrivals = Vec::new();
    let mut clock_offset_ms = 0i64;

    for stop_id in &stop_ids {
        println!("=== Stop {} ===", stop_id);
        let (arrivals, offset) = fetch_arrivals(stop_id).await?;
        clock_offset_ms = offset;
        for a in &arrivals {
            println!(
                "  {:20} {:>5}  route={:<15} trip={:<20} time_ms={}",
                a.destination,
                format_time(a.minutes),
                a.route_id,
                a.trip_id,
                a.arrival_time_ms
            );
        }
        all_arrivals.extend(arrivals);
        println!();
    }

    // Sort only (no filter/dedup yet) for the "before" view
    all_arrivals.sort_by_key(|a| a.arrival_time_ms);

    println!("=== Combined & Sorted (before dedup) ===");
    for a in &all_arrivals {
        println!(
            "  {:20} {:>5}  route={:<15} trip={:<20} stop={}",
            a.destination,
            format_time(a.minutes),
            a.route_id,
            a.trip_id,
            a.stop_id
        );
    }

    recalculate_and_filter(&mut all_arrivals, clock_offset_ms);

    println!("\n=== After dedup & filter ===");
    for a in &all_arrivals {
        println!(
            "  {:20} {:>5}  route={:<15} trip={}",
            a.destination,
            format_time(a.minutes),
            a.route_id,
            a.trip_id
        );
    }

    Ok(())
}
