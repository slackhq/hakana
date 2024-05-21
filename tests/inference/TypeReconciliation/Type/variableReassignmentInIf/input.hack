final class One {
    public function fooFoo(): void {}
}

final class Two {
    public function barBar(): void {}
}

$one = new One();

if (1 + 1 === 2) {
    $one = new Two();

    $one->barBar();
}