final class Request {
    <<\Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
    public static function getName() : string {
        return "";
    }
}


echo Request::getName();