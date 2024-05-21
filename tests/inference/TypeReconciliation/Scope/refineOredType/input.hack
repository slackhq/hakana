abstract class A {
    public function doThing(): void
    {
        if ($this is B || $this is C) {
            if ($this is B) {

            }
        }
    }
}
final class B extends A {}
final class C extends A {}