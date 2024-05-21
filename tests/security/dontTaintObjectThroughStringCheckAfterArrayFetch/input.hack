final class A {
    public function __construct(public string $s) {}
}

function pass_if_string(mixed $m): ?string {
    if ($m is string) {
        return $m;
    }

    return null;
}

function pass_once(): void {
    $a = new A($_GET["bad"]);
    pass_twice(dict["foo" => $a]);
}

function pass_twice(dict<string, A> $dict): void {
    echo pass_if_string($dict['foo']);
}