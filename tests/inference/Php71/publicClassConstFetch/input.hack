class A
{
    const IS_PUBLIC = 1;
    const IS_ALSO_PUBLIC = 2;
}

class B extends A
{
    function fooFoo(): int {
        echo A::IS_PUBLIC;
        return A::IS_ALSO_PUBLIC;
    }
}

echo A::IS_PUBLIC;
echo A::IS_ALSO_PUBLIC;