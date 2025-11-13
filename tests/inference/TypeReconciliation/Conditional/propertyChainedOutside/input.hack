final class Assessment {
    public ?string $root = null;
}

final class Project {
    public ?Assessment $assessment = null;
}

function f(Project $project): int {
    if (($project->assessment === null)
        || ($project->assessment->root === null)
    ) {
        throw new RuntimeException();
    }

    return HH\Lib\Str\length($project->assessment->root);
}