
/**
 * Not working
 */
function sinkNotWorking(<<\Hakana\SecurityAnalysis\Sink('html')>> $sink) : string {}

echo sinkNotWorking($_GET["taint"]);