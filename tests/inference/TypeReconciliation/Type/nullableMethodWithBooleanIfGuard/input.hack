class One {
    public function fooFoo(): void {}
}

class Two {
    public function fooFoo(): void {}
}

class B {
    public function barBar(?One $one = null, ?Two $two = null): void {
        if ($one && $two) {
            $two->fooFoo();
        }
    }
}