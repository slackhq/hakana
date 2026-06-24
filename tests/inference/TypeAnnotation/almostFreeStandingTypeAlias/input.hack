type CoolType = ?A;

final class A {}

function foo(): CoolType {
    if (rand(0, 1) !== 0) {
        return new A();
    }
    
    return null;
}

function bar(CoolType $a) : void { }

bar(foo());
