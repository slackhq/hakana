
/**
 * Not working
 */
function sinkNotWorking(<<\Hakana\SecurityAnalysis\Sink('HtmlTag')>> $sink) : string {}

echo sinkNotWorking($_GET["taint"]);