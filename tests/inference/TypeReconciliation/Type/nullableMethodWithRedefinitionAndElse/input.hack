class One {
    /** @var int|null */
    public $two;

    public function fooFoo(): void {}
}

class B {
    public function barBar(?One $one = null) : void {
        if (!$one) {
            $one = new One();
        }
        else {
            $one->two = 3;
        }

        $one->fooFoo();
    }
}