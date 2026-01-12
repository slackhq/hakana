namespace Bar;

function test_valid_keys(\Foo\allowed_logstash_keys_t $data): void {
    // These should be valid - keys exist in ALLOWED_LOGSTASH_KEYS
    echo $data['a'];  // valid key
    echo $data['b'];  // valid key 
    echo $data['c'];  // valid key
}

function test_underscore_keys(\Foo\allowed_logstash_keys_t $data): void {
    // These should be valid - keys start with '__'
    echo $data['__internal'];  // allowed by __ prefix
    echo $data['__debug'];     // allowed by __ prefix
}

function test_invalid_keys(\Foo\allowed_logstash_keys_t $data): void {
    // This should cause an error - key doesn't exist in constant
    echo $data['invalid_key']; // ERROR: should not be allowed
}

function test_type_mismatch(\Foo\allowed_logstash_keys_t $data): void {
    // With ShapeKeysFromConstant, type mismatches should be ignored
    // (unlike ShapeTypeFromConstant which would enforce types)
    
    // These access valid keys but with potentially wrong types - should be OK
    $int_val = (int) $data['a'];      // 'a' is TEXT in constant, but casting should be fine
    $str_val = (string) $data['b'];   // 'b' is LONG in constant, but casting should be fine
    $bool_val = (bool) $data['c'];    // 'c' is BOOLEAN in constant, should be fine
}