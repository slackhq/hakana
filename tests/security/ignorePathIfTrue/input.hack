<<Hakana\SecurityAnalysis\IgnorePathIfTrue()>>
function is_dev(): bool {
    return rand(0, 1) ? true : false;
}

function foo(): void {
    if (is_dev()) {
        $a = HH\global_get('_GET')['a'];
        echo $a;
    }

    if (is_dev() && rand(0, 1)) {
        $a = HH\global_get('_GET')['a'];
        echo $a;
    }

    if (rand(0, 1) && is_dev()) {
        $a = HH\global_get('_GET')['a'];
        echo $a;
    }
}