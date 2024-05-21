final class One {
    public function fooFoo(): void {}
}

final class Two {
    public function fooFoo(): void {}
}

final class B {
    public function barBar(?One $one = null, ?Two $two = null) : void {
        if ($one !== null && ($two || rand(0, 1))) {
            $one->fooFoo();
        }
    }
}