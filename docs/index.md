---
layout: default
title: QSMxT
nav_order: 1
---

<head>
  <link rel="stylesheet" href="https://maxcdn.bootstrapcdn.com/bootstrap/3.4.1/css/bootstrap.min.css">
  <script src="https://ajax.googleapis.com/ajax/libs/jquery/3.6.0/jquery.min.js"></script>
  <script src="https://maxcdn.bootstrapcdn.com/bootstrap/3.4.1/js/bootstrap.min.js"></script>
</head>

# QSMxT

QSMxT is an end-to-end software toolbox for QSM that automatically reconstructs and processes large datasets in parallel using sensible defaults.


QSMxT produces:

 - Quantitative Susceptibility Maps (QSM)
 - Anatomical segmentation in both the GRE/QSM and T1w spaces
 - Spreadsheets in CSV format with susceptibility statistics across brain regions of interest
 - A group space/template, including average QSM and GRE images across your cohort

QSMxT requires gradient-recalled echo (GRE) MR images converted to the Brain Imaging Data Structure (BIDS). QSMxT also includes tools to convert DICOM or NIfTI images to BIDS.

![QSMxT Process Diagram](/images/qsmxt-process-diagram.png)

[See an interactive notebook applying QSMxT here!](https://www.neurodesk.org/example-notebooks/structural_imaging/qsmxt_example.html)

## What is QSM?

<a href="https://doi.org/10.1002/nbm.3569" data-placement="top" data-toggle="popover" data-trigger="hover focus" target="_blank" data-content="Click to see Deistung et al. 'Overview of Quantitative Susceptibility Mapping'.">Quantitative Susceptibility Mapping</a> (QSM) is an emerging form of <a href="#" data-placement="top" data-toggle="popover" data-trigger="hover focus" data-content="Quantitative MRI measures a physical property rather than a signal strength, such that values are measured independently of scanner hardware or acquisition settings.">quantitative MRI</a> that aims to measure the <a href="#" data-placement="top" data-trigger="hover focus" data-toggle="popover" data-content="Magnetic susceptibility (χ; 'chi') is the degree to which an object becomes magnetised by an external magnetic field.">magnetic susceptibility</a> of objects. Susceptibility maps are derived by post-processing the phase component of the complex MRI signal from a T2*-weighted acquisition such as 3D gradient-echo (3D-GRE) or 3D echo planar imaging (3D-EPI). QSM has <a href="https://doi.org/10.1002/nbm.3569" data-placement="top" data-toggle="popover" data-trigger="hover focus" target="_blank" data-content="Click to see Deistung et al. 'Overview of Quantitative Susceptibility Mapping'.">many applications</a>, mostly in human brain imaging of conditions such as traumatic brain injuries, neuroinflammatory and neurodegenerative diseases, ageing, tumours, with emerging applications across the human body and in animals.

## How do I cite QSMxT?

Please cite our paper:

{: .highlight }
Stewart AW, Robinson SD, O’Brien K, Jin J, Widhalm G, Hangel G, Walls A, Goodwin J, Eckstein K, Tourell M, Morgan C. "QSMxT: Robust masking and artifact reduction for quantitative susceptibility mapping". *Magnetic resonance in medicine* 87.3 (2022): 1289-1300. https://doi.org/10.1002/mrm.29048

In addition, since each processing step automated by QSMxT uses a range of underlying technologies and software, we provide a `references.txt` file in the output directory which lists citations for the methods used. These citations adapt depending on your input data and parameters so they reflect the methods that were actually used.

## What algorithms does QSMxT use and how were they chosen?

See the [algorithms](/QSMxT/algorithms) page for the underlying algorithms used for each step.

Many QSM algorithms have been proposed in recent years, with each having unique advantages and disadvantages. However, most algorithms are written in languages that are difficult to automate across large and varied datasets, and/or require proprietary licensing. We chose algorithms implemented in languages that were possible to run within open-source and containerised environments.

## Can you include my preferred algorithm in QSMxT? 

If you are able to provide or point us to an implementation of a QSM algorithm in a language that can be run in a command-line environment, along with a justified use-case, we would gladly work with you to integrate it. Feel free to open an issue on GitHub with your request. We can also accept contributions in the form of pull requests to the GitHub repository if you are able to integrate it yourself. 

<script>
$(document).ready(function(){
    $('[data-toggle="popover"]').popover();   
});
$("[data-toggle=popover]")
.popover({html:true})
</script>

