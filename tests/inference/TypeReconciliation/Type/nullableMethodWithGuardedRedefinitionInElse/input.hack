final class One {
    public function fooFoo(): void {}
}

final class B {
    public function barBar(?One $one = null) : void {
        if ($one) {
            // do nothing
        }
        else {
            $one = new One();
        }

        $one->fooFoo();
    }
}