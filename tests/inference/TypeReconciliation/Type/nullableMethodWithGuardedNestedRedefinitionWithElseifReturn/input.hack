class One {
    public function fooFoo(): void {}
}

class B {
    public function barBar(?One $one = null) : void {
        $a = rand(0, 4);

        if ($one === null) {
            if ($a === 4) {
                $one = new One();
            }
            else if ($a === 3) {
                // do nothing
                return;
            }
            else {
                $one = new One();
            }
        }

        $one->fooFoo();
    }
}