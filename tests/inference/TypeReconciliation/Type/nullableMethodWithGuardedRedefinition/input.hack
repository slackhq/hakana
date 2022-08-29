class One {
    public function fooFoo(): void {}
}

class B {
    public function barBar(?One $one = null) : void {
        if ($one === null) {
            $one = new One();
        }

        $one->fooFoo();
    }
}