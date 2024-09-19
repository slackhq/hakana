function foo(): string {
    $a = () ==> {
        $b = HH\global_get('_GET')['b'];
        return $b;
    };
    return "c";
}

echo foo();