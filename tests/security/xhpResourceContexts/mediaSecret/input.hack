use type Facebook\XHP\HTML\img;
<<Hakana\SecurityAnalysis\Source('SystemSecret')>>
function secret(): string { return 'secret'; }
$image = <img src={secret()} />;
