function register(mixed $callback): void {}
function identity<T>(T $value): T { return $value; }

function unknown_write(): void {
    $ref = new \HH\Lib\Ref(null);
    register(($value) ==> {
        $ref->value = $value;
    });
    $value = identity($ref->value);
    if ($value is nonnull) {
        echo 'set';
    }
    $other_value = identity($ref->value);
    if ($other_value is null) {
        echo 'unset';
    }
}
