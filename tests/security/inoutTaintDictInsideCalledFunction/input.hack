function foo(inout dict<string, string> $d): void {
    if (rand(0, 1)) {
        $d["a"] = HH\global_get('_GET')["a"];
    }
}

function bar(): void {
    $my_dict = dict[];
    foo(inout $my_dict);
    echo_dict_key($my_dict);
}

function echo_dict_key(dict<string, string> $my_dict): void {
    echo $my_dict["a"];
}