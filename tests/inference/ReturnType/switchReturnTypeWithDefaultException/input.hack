class A {
    public function fooFoo(): bool {
        switch (rand(0,10)) {
            case 1:
            case 2:
                return true;

            default:
                throw new \Exception("badness");
        }
    }
}