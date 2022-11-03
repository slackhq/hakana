function foo(): void {}

$a = () ==> {};
$a();

$b = () ==> {
    foo();
};
$b();