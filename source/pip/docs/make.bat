@ECHO OFF

pushd %~dp0

REM Command file for Sphinx documentation

if "%SPHINXBUILD%" == "" (
	set SPHINXBUILD=sphinx-build
)
set SOURCEDIR=.
set BUILDDIR=_build

%SPHINXBUILD% >NUL 2>NUL
if errorlevel 9009 (
	echo.
	echo.The 'sphinx-build' command was not found. Install Sphinx and the docs
	echo.requirements with:
	echo.
	echo.   pip install -r requirements.txt
	echo.
	exit /b 1
)

if "%1" == "" goto help
if "%1" == "html" goto html
if "%1" == "clean" goto clean

:help
%SPHINXBUILD% -M help %SOURCEDIR% %BUILDDIR% %SPHINXOPTS% %O%
goto end

:html
%SPHINXBUILD% -M html %SOURCEDIR% %BUILDDIR% %SPHINXOPTS% %O%
if errorlevel 1 exit /b 1
echo.
echo.Build finished. HTML pages are in %BUILDDIR%\html.
goto end

:clean
rmdir /s /q %BUILDDIR%
echo.Build directory cleaned.
goto end

:end
popd
