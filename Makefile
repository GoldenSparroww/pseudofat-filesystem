APP := pseudofat

ifeq ($(OS),Windows_NT)
	TARGET_DIR := dist
	TARGET_BIN := $(TARGET_DIR)\$(APP).exe
	MKDIR := if not exist $(TARGET_DIR) mkdir $(TARGET_DIR)
	RMDIR := if exist $(TARGET_DIR) rmdir /s /q $(TARGET_DIR) & if exist target rmdir /s /q target
	COPY := copy /Y
	BUILD_BIN := target\release\$(APP).exe
else
	TARGET_DIR := dist
	TARGET_BIN := $(TARGET_DIR)/$(APP)
	MKDIR := mkdir -p $(TARGET_DIR)
	RMDIR := rm -rf $(TARGET_DIR) target
	COPY := cp -f
	BUILD_BIN := target/release/$(APP)
endif

.PHONY: all build test run clean

all: build

build b: clean
	cargo build --release
	$(MKDIR)
	$(COPY) $(BUILD_BIN) $(TARGET_BIN)

test t:
	cargo test

run r: build
	$(TARGET_BIN) ./dist/myfs.dat

clean c:
	cargo clean
	$(RMDIR)