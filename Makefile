setplugin:
	cp plugin_sample/target/wasm32-unknown-unknown/debug/plugin_sample.wasm \
		./rust_editor/plugin.wasm

.PHONY: setplugin
