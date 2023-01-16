class A {}

function foo(): A {
    switch (rand(0,1)) {
        case true:
            return new A();
            break;
        default:
            return new A();
    }
}