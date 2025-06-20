package main

import (
	"context"
	"dagger/http-nu/internal/dagger"
)

type HttpNu struct{}

func (m *HttpNu) Hello(ctx context.Context) string {
	return "Hello"
}

func (m *HttpNu) Upload(
	ctx context.Context,
	// +ignore=["**", "!Cargo.toml", "!Cargo.lock", "!src/**", "!xs.nu", "!scripts/**"]
	src *dagger.Directory) *dagger.Container {
	return dag.Container().
		From("joseluisq/rust-linux-darwin-builder:latest").
		// mount Rust project
		WithMountedDirectory("/app", src).
		WithWorkdir("/app")
}


