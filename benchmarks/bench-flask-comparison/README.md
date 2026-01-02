# Benchmark: Flask + gunicorn

Baseline benchmark for Flask served via gunicorn, for comparison against
http-nu.

## Requirements

```bash
pip install flask gunicorn
```

## Run

```bash
nu run.nu              # run and display
nu run.nu --save       # run and save to results.nuon
nu run.nu -w 8         # use 8 gunicorn workers
nu run.nu -c 100 -d 10s  # 100 connections, 10 second duration
```

## Results

See `results.nuon`
