final class One {
    public function fooFoo(): void {}
}

final class Two {
    public function fooFoo(): void {}
}

final class B {
    public function barBar(?One $one = null) : void {
        $a = rand(0, 4);

        if ($one === null) {
            if ($a === 4) {
                $one = new One();
            }
            else {
                $one = new One();
            }
        }

        $one->fooFoo();
    }
}