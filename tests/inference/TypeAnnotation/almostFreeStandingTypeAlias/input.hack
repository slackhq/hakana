type CoolType = ?A;

class A {}

function foo(): CoolType {
    if (rand(0, 1)) {
        return new A();
    }
    
    return null;
}

function bar(CoolType $a) : void { }

bar(foo());