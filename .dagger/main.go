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
	src *dagger.Directory) *dagger.Directory {
		return src
}


func (m *HttpNu) CrossBuilderOsx(
	ctx context.Context,
	src *dagger.Directory) *dagger.Container {


	return dag.Container().
		From("joseluisq/rust-linux-darwin-builder:latest").

		// mount Rust project
		WithMountedDirectory("/app", src).
		WithWorkdir("/app")

	// Build your app
	// WithExec([]string{"cargo", "build", "--release", "--target", "aarch64-apple-darwin"})
}



func (m *HttpNu) CrossBuilderWindows(
	ctx context.Context,
	src *dagger.Directory) *dagger.Container {
	return dag.Container().
		From("joseluisq/rust-linux-darwin-builder:latest").

		// install Windows cross-compilation tools
		WithExec([]string{"apt", "update"}).
		WithExec([]string{"apt", "install", "-y", "nasm", "gcc-mingw-w64-i686", "mingw-w64", "mingw-w64-tools"}).

		// add Windows target
		WithExec([]string{"rustup", "target", "add", "x86_64-pc-windows-gnu"}).

		// set cross-compilation environment variables
		WithEnvVariable("CC_x86_64_pc_windows_gnu", "x86_64-w64-mingw32-gcc").
		WithEnvVariable("CXX_x86_64_pc_windows_gnu", "x86_64-w64-mingw32-g++").
		WithEnvVariable("AR_x86_64_pc_windows_gnu", "x86_64-w64-mingw32-gcc-ar").
		WithEnvVariable("DLLTOOL_x86_64_pc_windows_gnu", "x86_64-w64-mingw32-dlltool").
		WithEnvVariable("CFLAGS_x86_64_pc_windows_gnu", "-m64").
		WithEnvVariable("ASM_NASM_x86_64_pc_windows_gnu", "/usr/bin/nasm").
		WithEnvVariable("AWS_LC_SYS_PREBUILT_NASM", "0").

		// mount Rust project
		WithMountedDirectory("/app", src).
		WithWorkdir("/app")
}



