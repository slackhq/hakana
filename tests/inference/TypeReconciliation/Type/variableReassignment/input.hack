final class One {
    public function fooFoo(): void {}
}

final class Two {
    public function barBar(): void {}
}

$one = new One();

$one = new Two();

$one->barBar();