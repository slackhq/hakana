function foo() : void {
    $a = "9" . "a" . "b" . "c" . ((string) HH\global_get('_GET')["bad"]) . "d" . "e" . "f";
    exec($a);
}