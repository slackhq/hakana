class One {
    public function fooFoo(): void {}
}

class B {
    public function barBar(?One $one = null) : void {
        $a = 4;

        switch ($a) {
            case 4:
                if ($one === null) {
                    break;
                }

                $one->fooFoo();
                break;
        }
    }
}