function foo(): string {
    $a = () ==> {
        $b = $_GET['b'];
        return $b;
    };
    return "c";
}

echo foo();