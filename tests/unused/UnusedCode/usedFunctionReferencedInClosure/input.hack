function foo(): void {}

<<__EntryPoint>>
function bar() {
    $a = () ==> {};
    $a();

    $b = () ==> {
        foo();
    };
    $b();
}
