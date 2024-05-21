final class One {
    public function fooFoo(): void {}
}

final class B {
    public function barBar(?One $one = null) : void {
        $a = rand(0, 1) ? 3 : 4;

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