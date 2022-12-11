#!/bin/bash

# Builds WASM

npm install
wasm-pack build
npm run build