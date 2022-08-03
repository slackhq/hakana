class A {
    public function doThing(): void
    {
        if ($this is B || $this is C) {
            if ($this is B) {

            }
        }
    }
}
class B extends A {}
class C extends A {}