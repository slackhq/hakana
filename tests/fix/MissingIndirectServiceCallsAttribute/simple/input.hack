<<\Hakana\CallsService('MyService')>>
function calls_service(): void {
    echo "calling service";
}

function caller(): void {
    calls_service();
}
