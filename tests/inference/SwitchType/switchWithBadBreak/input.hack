final class A {}

function foo(): A {
    switch (rand(0,1)) {
        case 1:
            return new A();
            break;
        default:
            return new A();
    }
}