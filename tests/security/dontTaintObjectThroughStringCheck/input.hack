final class A {
    public function __construct(public string $s) {}
}

function pass_if_string(mixed $m): ?string {
    if ($m is string) {
        return $m;
    }

    return null;
}

function do_echo(): void {
    $a = new A(HH\global_get('_GET')["bad"]);
    echo pass_if_string($a);
}
