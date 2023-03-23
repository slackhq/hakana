function foo(): bool {
    $ref = new HH\Lib\Ref(false);

    $a = (bool $b) ==> {
        $ref->value = $b;
    };

    $a(true);

    if ($ref->value) {}

    return $ref->value;
}