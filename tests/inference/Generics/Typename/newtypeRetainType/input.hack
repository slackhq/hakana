function takes_deeper_foo(deeper_foo $foo): void {}

function coerce_to_type<T>(mixed $m, typename<T> $type): T {}

function bar(string $s) {
    $t = coerce_to_type($s, deeper_foo::class);
    takes_deeper_foo($t);
}