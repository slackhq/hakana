final class One {
    public string $a = "";

    public function fooFoo(): void {}
}

final class Two {
    public function fooFoo(): void {}
}

final class B {
    public function barBar(?One $one = null, ?Two $two = null): void {
        if ($one === null) {
            return;
        }

        if (!$one->a && $one->fooFoo()) {
            // do something
        }
    }
}