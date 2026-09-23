use type Facebook\XHP\HTML\a;
<<Hakana\SecurityAnalysis\Source('SystemSecret')>>
function secret(): string { return 'secret'; }
function render(string $id): void {
    $query = secret();
    $link = <a href={'/apps/'.$id.'?'.$query} />;
}
