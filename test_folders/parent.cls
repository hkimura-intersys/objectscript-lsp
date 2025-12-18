Class MyApp.Parent
{

ClassMethod Hello() As %String
{
    Quit "Hello from Parent"
}

ClassMethod OnlyInParent() As %String
{
    Quit "This exists only on Parent"
}

}
