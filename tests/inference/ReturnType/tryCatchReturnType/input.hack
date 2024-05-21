final class A {
    public function fooFoo(): bool {
        try {
            // do a thing
            return true;
        }
        catch (\Exception $e) {
            throw $e;
        }
    }
}