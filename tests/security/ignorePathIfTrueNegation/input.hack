<<Hakana\SecurityAnalysis\IgnorePathIfTrue()>>
function is_dev(): bool {
    return rand(0, 1) ? true : false;
}

function foo(): void {
    if (!is_dev()) {
        return;
    }

    $a = HH\global_get('_GET')['a'];
    echo $a;
}