class Route implements HH\ClassAttribute {
    public function __construct(private vec<string> $methods = vec[]) {}
}
<<Route(vec["GET"])>>
class HealthController {}