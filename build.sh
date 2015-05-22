set -ex
cd `dirname $0`

build() {
	TARGET=$1
	# Set path
	_PATH=$PATH
	case "$TARGET" in
		"i686-pc-windows" ) 
			RUST=$TOOLS/rust-nightly-$TARGET-gnu
			export PATH=$PATH:/mingw32/bin/
			export OPENSSL_LIBS=crypto:ssl
			;;
		"x86_64-pc-windows" ) 
			RUST=$TOOLS/rust-nightly-$TARGET-gnu
			export PATH=$PATH:/mingw64/bin/
			export OPENSSL_LIBS=crypto:ssl
			;;
		* )
			RUST=
			;;
	esac
	
	if [ "$RUST" != "" ] && [ -d "$RUST" ]; then
		export PATH=$PATH:$RUST/rustc/bin/:$RUST/cargo/bin/
	fi

	# Build
	rustc --version
	cargo version
	cargo test
	cargo build --release
	
	# Prepare for installer
	if [ "$TARGET" == "i686-pc-windows" ]; then
		cp $RUST/rustc/bin/libgcc*.dll target/release/
	fi
	
	# Build installer
	if [ "$WIXSHARP_DIR" != "" ]; then
		$WIXSHARP_DIR/cscs wixcs/setup.cs
		nuget pack target/octobuild.nuspec -OutputDirectory target
	fi
	
	# Restore path
	export PATH=$_PATH
}

if [ "$1" != "" ]; then
	# User defined target
	case $1 in
		"i686" )
			build i686-pc-windows
			;;
		"x86_64" )
			build x86_64-pc-windows
			;;
		"windows" )
			# Windows build
			build i686-pc-windows
			build x86_64-pc-windows
			;;
		* )
			build $1
			;;
	esac
elif [ "$ProgramW6432" != "" ]; then
	# Windows build
	build x86_64-pc-windows
elif [ "$ProgramData" != "" ]; then
	# Windows build
	build i686-pc-windows
else 
	# Linux build
	build x86_64-unknown-linux
fi
