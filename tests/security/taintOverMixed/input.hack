function foo() : void {
    $a = HH\global_get('_GET')["bad"];
    echo $a;
}