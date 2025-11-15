namespace Bar;

function foo(\Foo\allowed_logstash_keys_t $t): void {
    echo $t['key_does_not_exist'];
}