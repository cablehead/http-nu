package main

import (
	"context"
	"dagger/http-nu/internal/dagger"
)

type HttpNu struct{}

func (m *HttpNu) withCaches(container *dagger.Container, targetSuffix string) *dagger.Container {
	// Separate caches per target
	registryCache := dag.CacheVolume("dagger-cargo-registry-" + targetSuffix)
	gitCache := dag.CacheVolume("dagger-cargo-git-" + targetSuffix)
	targetCache := dag.CacheVolume("dagger-cargo-target-" + targetSuffix)

	return container.
		WithMountedCache("/root/.cargo/registry", registryCache).
		WithMountedCache("/root/.cargo/git", gitCache).
		WithMountedCache("/app/target", targetCache)
}

func (m *HttpNu) Upload(
	ctx context.Context,
	// +ignore=["**", "!Cargo.toml", "!Cargo.lock", "!src/**", "!xs.nu", "!scripts/**"]
	src *dagger.Directory) *dagger.Directory {
	return src
}

func (m *HttpNu) DarwinEnv(
	ctx context.Context,
	src *dagger.Directory) *dagger.Container {
	return m.withCaches(
		dag.Container().
			From("joseluisq/rust-linux-darwin-builder:latest").
			WithMountedDirectory("/app", src).
			WithWorkdir("/app"),
		"darwin-arm64",
	)
}

func (m *HttpNu) DarwinBuild(ctx context.Context, src *dagger.Directory) *dagger.File {
	return m.DarwinEnv(ctx, src).
		WithExec([]string{"./scripts/cross-build-darwin.sh", "--release"}).
		WithExec([]string{"tar", "-czf", "/tmp/http-nu-darwin-arm64.tar.gz", "-C", "/app/target/aarch64-apple-darwin/release", "http-nu"}).
		File("/tmp/http-nu-darwin-arm64.tar.gz")
}

func (m *HttpNu) WindowsEnv(
	ctx context.Context,
	src *dagger.Directory) *dagger.Container {
	return m.withCaches(
		dag.Container().
			From("joseluisq/rust-linux-darwin-builder:latest").
			WithExec([]string{"apt", "update"}).
			WithExec([]string{"apt", "install", "-y", "nasm", "gcc-mingw-w64-i686", "mingw-w64", "mingw-w64-tools"}).
			WithExec([]string{"rustup", "target", "add", "x86_64-pc-windows-gnu"}).
			WithEnvVariable("CARGO_BUILD_TARGET", "x86_64-pc-windows-gnu").
			WithEnvVariable("CC_x86_64_pc_windows_gnu", "x86_64-w64-mingw32-gcc").
			WithEnvVariable("CXX_x86_64_pc_windows_gnu", "x86_64-w64-mingw32-g++").
			WithEnvVariable("AR_x86_64_pc_windows_gnu", "x86_64-w64-mingw32-gcc-ar").
			WithEnvVariable("DLLTOOL_x86_64_pc_windows_gnu", "x86_64-w64-mingw32-dlltool").
			WithEnvVariable("CFLAGS_x86_64_pc_windows_gnu", "-m64").
			WithEnvVariable("ASM_NASM_x86_64_pc_windows_gnu", "/usr/bin/nasm").
			WithEnvVariable("AWS_LC_SYS_PREBUILT_NASM", "0").
			WithMountedDirectory("/app", src).
			WithWorkdir("/app"),
		"windows-amd64",
	)
}

func (m *HttpNu) WindowsBuild(ctx context.Context, src *dagger.Directory) *dagger.File {
	return m.WindowsEnv(ctx, src).
		WithExec([]string{"cargo", "build", "--release"}).
		WithExec([]string{"tar", "-czf", "/tmp/http-nu-windows-amd64.tar.gz", "-C", "/app/target/x86_64-pc-windows-gnu/release", "http-nu.exe"}).
		File("/tmp/http-nu-windows-amd64.tar.gz")
}

func (m *HttpNu) LinuxArm64Env(
	ctx context.Context,
	src *dagger.Directory) *dagger.Container {
	return m.withCaches(
		dag.Container().
			From("messense/rust-musl-cross:aarch64-musl").
			WithMountedDirectory("/app", src).
			WithWorkdir("/app"),
		"linux-arm64",
	)
}

func (m *HttpNu) LinuxArm64Build(ctx context.Context, src *dagger.Directory) *dagger.File {
	return m.LinuxArm64Env(ctx, src).
		WithExec([]string{"cargo", "build", "--release", "--target", "aarch64-unknown-linux-musl"}).
		WithExec([]string{"tar", "-czf", "/tmp/http-nu-linux-arm64.tar.gz", "-C", "/app/target/aarch64-unknown-linux-musl/release", "http-nu"}).
		File("/tmp/http-nu-linux-arm64.tar.gz")
}

func (m *HttpNu) LinuxAmd64Env(
	ctx context.Context,
	src *dagger.Directory) *dagger.Container {
	return m.withCaches(
		dag.Container().
			From("messense/rust-musl-cross:x86_64-musl").
			WithMountedDirectory("/app", src).
			WithWorkdir("/app"),
		"linux-amd64",
	)
}

func (m *HttpNu) LinuxAmd64Build(ctx context.Context, src *dagger.Directory) *dagger.File {
	return m.LinuxAmd64Env(ctx, src).
		WithExec([]string{"cargo", "build", "--release", "--target", "x86_64-unknown-linux-musl"}).
		WithExec([]string{"tar", "-czf", "/tmp/http-nu-linux-amd64.tar.gz", "-C", "/app/target/x86_64-unknown-linux-musl/release", "http-nu"}).
		File("/tmp/http-nu-linux-amd64.tar.gz")
}
