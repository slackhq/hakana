
/**
 * Not working
 */
function sinkNotWorking(<<\Hakana\SecurityAnalysis\Sink('HtmlTag')>> $sink) : string {}

echo sinkNotWorking(HH\global_get('_GET')["taint"]);