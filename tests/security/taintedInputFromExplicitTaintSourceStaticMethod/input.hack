class Request {
    <<\Hakana\SecurityAnalysis\Source("input")>>
    public static function getName() : string {
        return "";
    }
}


echo Request::getName();