# LUT-Utility

A utility to 
- **generate** identity/neutral LUT images of different types (HALD CLUT or Unwrapped Cube as used in ReShade) as base for your edits
- **convert** these types of LUT images to .cube LUTs of different sizes (smaller or equal than the input size) and to
- **apply** these LUTs to images.

A HALD CLUT looks like this:

<img src="https://github.com/user-attachments/assets/c1862b72-370a-4929-8a9b-192c6a38a256" width="128">

A Reshade LUT looks like this:
![uc_identity_32](https://github.com/user-attachments/assets/af6199d5-da72-477b-b025-0820fb571b38)


## Use-case 1: Export your edited looks/grades (from any editing software) as .cube
You may have a camera like the Lumix S9 or some other software that requires .cube LUTs of a specific size (e.g. 33) for image and movie grading.

###
1. Create an identity HALD image:
~~~sh
Usage: lut_utility.exe generate [OPTIONS] --format <FORMAT> --cube <CUBE> --output <OUTPUT>
Example: lut_utility generate -f H -c 144 -o identity144.png

Options:
  -f, --format <FORMAT>        The format of the identity LUT to generate (hald or unwrapped-cube) [possible values: hald, unwrapped-cube]
  -c, --cube <CUBE>            The base cube size for LUT generation (is same as level^2 for HALD). Affects density and image size: Image dimension = HALD: √(cube³) x √(cube³) UNWRAPPED CUBE: cube² x cube. Practical range: HALD: 1-400 UNWRAPPED CUBE 1-64. Higher values generate very large images
  -o, --output <OUTPUT>        Output filename for the generated PNG image. Must have a .png extension
  -b, --bit-depth <BIT_DEPTH>  Desired bit depth for the output PNG (8 or 16). Only applicable for Unwrapped Cube format [default: 8]
~~~

2. Use this generated png in your editing software to apply your looks and grades. Note:
- Only color grading and brightness changes can be applied to this HALD image.
- **sharpening**, **bluring**, **grain**, **lens corrections**, **noise reduction**, **cropping** will destroy your HALD image and can NOT be uses as LUT.

3. Save the image with your grades applied to a lossless PNG "mygrade-hald144.png"

4. Convert your graded HALD image to .cube
~~~sh
Usage: lut_utility.exe convert [OPTIONS] --input <INPUT> --output <OUTPUT>
Example: lut_utility convert -t 32 -i mygrade-hald144.png -o mygrade_cube32.cube

Options:
  -i, --input <INPUT>              Input LUT image filename (.png). Dimensions will determine LUT type (HALD or UNWRAPPED_CUBE)
  -o, --output <OUTPUT>            Output .cube filename (.cube)
  -t, --target-cube <TARGET_CUBE>  Optional target size for the output .cube file (e.g., 32, 33, 64). Must be <= the input LUT's native cube size
~~~

5. or preview your grade on any image
~~~sh
Usage: lut_utility.exe apply --lut <LUT> --input <INPUT> --output <OUTPUT>
Example: lut_utility apply -l mygrade-hald144.png -i my-ungraded-image.jpg -o mygraded-image-with-mylook1.png

Options:
  -l, --lut <LUT>        Path to the LUT file (.png or .cube)
  -i, --input <INPUT>    Path to the input image file (.jpg or .png)
  -o, --output <OUTPUT>  Path to save the output image file (.png)
~~~

## Use-case 2: Batch convert/apply alot of downloaded luts (New with v0.2.0)
This will convert all LUTs in input folder to cube LUTs in output folder preserving input filename and directory structure.
~~~sh
Usage: lut_utility convert [OPTIONS] --input <INPUT> --output <OUTPUT>
Example: lut_utility convert -i input\ReShadeLUTs -o output\CubeLUTs -t 32 -b

Options:
  -i, --input <INPUT>              Input LUT image file (.png) or a directory containing .png files for batch conversion
  -o, --output <OUTPUT>            Output .cube filename (.cube) for single file conversion, or output directory for batch conversion
  -t, --target-cube <TARGET_CUBE>  Optional target size for the output .cube file (e.g., 32, 33, 64). Must be <= the input LUT's native cube size
  -b, --batch                      Enable batch processing mode. Input and output must be directories
~~~

This will apply all LUTs in LUT (-l) folder to this one image (0424406328.jpg). The files will be stored as PNG in output\0424406328\ preserving LUT filename and directory structure.
~~~sh
Usage: lut_utility apply [OPTIONS] --lut <LUT> --input <INPUT> --output <OUTPUT>
Example: lut_utility apply -l input\HaldCLUTs -i input\0424406328.jpg -o output\ -b

Options:
  -l, --lut <LUT>        Path to the LUT file (.png or .cube) or a directory containing LUT files for batch application
  -i, --input <INPUT>    Path to the input image file (.jpg or .png). This remains a single file in batch mode
  -o, --output <OUTPUT>  Path to save the output image file (.png) for single LUT application, or output directory for batch application
  -b, --batch            Enable batch processing mode. LUT input must be a directory, output must be a directory
~~~

# Links
## Public free LUTs:
- https://github.com/cedeber/hald-clut
- https://gmic.eu/color_presets
- https://www.on1.com/free/luts/
- https://www.cyberpunk2077mod.com/cyberpunk-lut-pack/
- https://lut.tgratzer.com/

## Online Tools
- https://lut.tgratzer.com/
- https://www.color.io/free-online-lut-converter
- https://colorizer.net/index.php?op=3dlut_tools
- https://cameramanben.github.io/LUTCalc/LUTCalc/index.html

## Specs
- https://www.quelsolaar.com/technology/clut.html

# License
MIT License

Copyright (c) 2025 Christian Schrinner

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
