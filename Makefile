NAME=egress
RELEASE_DIR=release

.PHONY: all clean

TARGET_LIST =  \
aarch64-unknown-linux-musl \
aarch64-unknown-linux-gnu  \
aarch64-unknown-linux-musl \
x86_64-apple-darwin        \
x86_64-pc-windows-gnu      \
x86_64-unknown-freebsd     \
x86_64-unknown-linux-gnu   \
x86_64-unknown-linux-musl

all:
	@echo "Building release target for each platform..."
	for platform in $(TARGET_LIST); do \
		echo "Building for $${platform}..."; \
		cross build --release --target $${platform}; \
	done

release: all
	sh scripts/release.sh $(NAME) $(RELEASE_DIR) $(TARGET_LIST)

clean:
	@echo "Cleaning up..."
	cargo clean
