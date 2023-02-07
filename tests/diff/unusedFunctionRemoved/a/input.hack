<<__EntryPoint>>
function main(): void {
    $a = () ==> {
        foo();
    };
    $a();
    some_other_caller();
}