class A
{
    const IS_PROTECTED = 1;
}

class B extends A
{
    function fooFoo(): int {
        return A::IS_PROTECTED;
    }
}