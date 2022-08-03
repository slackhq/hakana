class A {
    public function fooFoo(): bool {
        switch (rand(0,10)) {
            case 1:
            default:
                return true;
        }
    }
}