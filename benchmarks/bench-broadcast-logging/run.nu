#!/usr/bin/env nu

# Benchmark script for broadcast-based logging

def main [
    --duration (-d): string = "5s"  # Duration per test
    --connections (-c): int = 50    # Concurrent connections
    --save (-s)                     # Save results to results.nuon
] {
    let script_dir = ($env.FILE_PWD? | default ".")
    let root = $"($script_dir)/../.."

    print "Building release..."
    cargo build --release --quiet

    mut results = []

    for format in ["jsonl", "human"] {
        let port = 9990 + ($results | length)
        let logfile = $"/tmp/http-nu-bench-($port).log"
        print $"Testing ($format) on :($port)..."

        # Start server, capture output to check for lag
        bash -c $"($root)/target/release/http-nu --log-format ($format) :($port) '{|req| {body: \"hello\"}}' > ($logfile) 2>&1 &"
        sleep 1sec

        # Run benchmark
        let oha_out = (oha -z $duration -c $connections $"http://127.0.0.1:($port)/" | complete).stdout

        # Kill server
        pkill -f $"http-nu.*:($port)"
        sleep 500ms

        # Parse benchmark results
        let rps = ($oha_out | parse -r 'Requests/sec:\s+([\d.]+)' | get 0?.capture0? | default "0" | into float)
        let avg = ($oha_out | parse -r 'Average:\s+([\d.]+)\s+ms' | get 0?.capture0? | default "0" | into float)

        # Check for lag (jsonl: "lagged" in JSON, human: "logging lagged" on stderr)
        let dropped = if $format == "jsonl" {
            let lagged = (open $logfile | lines | where {|l| $l =~ '"message":"lagged"'} | each {|l| $l | from json | get dropped})
            if ($lagged | is-empty) { 0 } else { $lagged | math sum }
        } else {
            let count = (grep -c "logging lagged" $logfile | complete | get stdout | str trim)
            if ($count | is-empty) { 0 } else { $count | into int }
        }

        $results = ($results | append {
            format: $format
            requests_per_sec: ($rps | math round -p 2)
            avg_latency_ms: ($avg | math round -p 2)
            dropped: $dropped
            connections: $connections
            duration: $duration
        })

        rm -f $logfile
    }

    if $save {
        $results | to nuon | save -f $"($script_dir)/results.nuon"
        print $"Saved to ($script_dir)/results.nuon"
    }

    $results
}
