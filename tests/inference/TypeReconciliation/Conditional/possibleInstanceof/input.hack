interface I1 {}
interface I2 {}

class A
{
    public function foo(): void {
        if ($this is I1 || $this is I2) {}
    }
}