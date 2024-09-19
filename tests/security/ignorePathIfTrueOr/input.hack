<<Hakana\SecurityAnalysis\IgnorePathIfTrue()>>
function is_dev(): bool {
    return rand(0, 1) ? true : false;
}

function foo(): void {
    if (rand(0, 1) || is_dev()) {
        $a = HH\global_get('_GET')['a'];
        echo $a;
    }
}