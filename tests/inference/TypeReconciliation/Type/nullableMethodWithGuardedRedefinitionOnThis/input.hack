final class One {
    public function fooFoo(): void {}
}

final class B {
    public ?One $one = null;

    public function barBar() : void {
        if ($this->one === null) {
            $this->one = new One();
        }

        $this->one->fooFoo();
    }
}