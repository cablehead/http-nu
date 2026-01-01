#!/usr/bin/env nu

# Benchmark: Flask + gunicorn (various configurations)

def main [
    --duration (-d): string = "5s"  # Duration per test
    --connections (-c): int = 50    # Concurrent connections
    --save (-s)                     # Save results to results.nuon
] {
    let script_dir = ($env.FILE_PWD? | default ".")
    let cores = (nproc | into int)
    mut results = []

    cd $script_dir

    # Test configurations: [workers, worker_type]
    let configs = [
        [4, "sync"]
        [$cores, "sync"]
        [4, "gevent"]
        [$cores, "gevent"]
        [($cores * 2), "gevent"]
    ]

    for config in $configs {
        let workers = $config.0
        let worker_type = $config.1
        let port = 9991 + ($results | length)

        print $"Testing Flask+gunicorn: ($workers) ($worker_type) workers on :($port)..."

        bash -c $"gunicorn -w ($workers) -k ($worker_type) -b 127.0.0.1:($port) 'app:app' > /dev/null 2>&1 &"
        sleep 2sec

        let oha_out = (oha -z $duration -c $connections $"http://127.0.0.1:($port)/" | complete).stdout
        pkill -f $"gunicorn.*($port)"
        sleep 500ms

        let rps = ($oha_out | parse -r 'Requests/sec:\s+([\d.]+)' | get 0?.capture0? | default "0" | into float)
        let avg = ($oha_out | parse -r 'Average:\s+([\d.]+)\s+ms' | get 0?.capture0? | default "0" | into float)

        $results = ($results | append {
            server: "flask-gunicorn"
            worker_type: $worker_type
            workers: $workers
            requests_per_sec: ($rps | math round -p 2)
            avg_latency_ms: ($avg | math round -p 2)
            connections: $connections
            duration: $duration
        })
    }

    print ""
    print "=== Results ==="
    $results | select worker_type workers requests_per_sec avg_latency_ms | table

    if $save {
        $results | to nuon | save -f $"($script_dir)/results.nuon"
        print $"Saved to ($script_dir)/results.nuon"
    }

    $results
}
