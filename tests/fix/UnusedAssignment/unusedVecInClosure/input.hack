function foo(): void {
    $fn = () ==> {
        $a = vec[];
        $a[] = rand(0, 1);
    };
    $fn();
}