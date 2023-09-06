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

# QSMxT: Automated and Scalable QSM

QSMxT is an *end-to-end* pipeline that automates the reconstruction, segmentation and analysis of QSM data across large groups of participants, from scanner images (DICOMs) through to susceptibility maps and quantitative outputs.

![QSMxT Process Diagram](/images/qsmxt-process-diagram.png)

## What is QSM?

<a href="https://doi.org/10.1002/nbm.3569" data-placement="top" data-toggle="popover" data-trigger="hover focus" target="_blank" data-content="Click to see Deistung et al. 'Overview of Quantitative Susceptibility Mapping'.">Quantitative Susceptibility Mapping</a> (QSM) is an emerging form of <a href="#" data-placement="top" data-toggle="popover" data-trigger="hover focus" data-content="Quantitative MRI measures a physical property rather than a signal strength, such that values are measured independently of scanner hardware or acquisition settings.">quantitative MRI</a> that aims to measure the <a href="#" data-placement="top" data-trigger="hover focus" data-toggle="popover" data-content="Magnetic susceptibility (χ; 'chi') is the degree to which an object becomes magnetised by an external magnetic field.">magnetic susceptibility</a> of objects. Susceptibility maps are derived by post-processing the phase component of the complex MRI signal from a T2*-weighted acquisition such as 3D gradient-echo (3D-GRE) or 3D echo planar imaging (3D-EPI). QSM has <a href="https://doi.org/10.1002/nbm.3569" data-placement="top" data-toggle="popover" data-trigger="hover focus" target="_blank" data-content="Click to see Deistung et al. 'Overview of Quantitative Susceptibility Mapping'.">many applications</a>, mostly in human brain imaging of conditions such as traumatic brain injuries, neuroinflammatory and neurodegenerative diseases, ageing, tumours, with emerging applications across the human body and in animals.

## What does QSMxT do?

QSMxT automates all tasks to include QSM in a study from data preparation and conversion to exporting susceptibility values across anatomical regions of interest. More specifically, QSMxT provides pipelines to automate the following tasks:

 - Data conversion (DICOM/NIfTI to <a href="https://bids.neuroimaging.io/" target="_blank" data-placement="top" data-toggle="popover" data-trigger="hover focus" data-content="Click to read about BIDS at bids.neuroimaging.io.">Brain Imaging Data Structure (BIDS)</a>)
 - QSM reconstruction (requires T2*-weighted magnitude and phase images)
 - T1 and QSM segmentation
 - Template building (requires T2*-weighted magnitude and QSM images)
 - Statistical data export to CSV (requires segmented QSM images)

QSMxT bundles a wide range of dependencies for QSM processing using software containerisation technology, making it extremely <a href="#" data-placement="top" data-toggle="popover" data-trigger="hover focus" data-content="Easy to access and install on your available hardware.">deployable</a> and <a href="#" data-placement="top" data-toggle="popover" data-trigger="hover focus" data-content="Producing the same results irrespective of computational environment, including hardware and software.">computationally reproducible</a>. The wide variety of dependencies that QSMxT uses can otherwise be challenging to install in a reproducible way, especially for non-developers and non-Linux users.

The <a href="https://nipype.readthedocs.io/en/latest/" data-placement="top" data-toggle="popover" data-trigger="hover focus" target="_blank" data-content="Click to read more at nipype.readthedocs.io">nipype</a> package is used to automate QSMxT's processing and make it scalable. Nipype is a workflow engine that can interact with a wide range of neuroimaging software, and provides straightforward scalability across jobs using an asynchronous directed graph data structure. This makes the automated processing of large datasets feasible, especially with high-performance computing systems (HPCs).

## How do I cite QSMxT?

Please cite our paper:

Stewart AW, Robinson SD, O’Brien K, Jin J, Widhalm G, Hangel G, Walls A, Goodwin J, Eckstein K, Tourell M, Morgan C. "QSMxT: Robust masking and artifact reduction for quantitative susceptibility mapping". *Magnetic resonance in medicine* 87.3 (2022): 1289-1300. https://doi.org/10.1002/mrm.29048

In addition, since each processing step automated by QSMxT uses a range of underlying technologies and software, we provide a `details_and_citations.txt` file in the output directory which lists citations for the methods used. These citations adapt depending on your input data and parameters so they reflect the methods that were actually used.

## What algorithms does QSMxT use and how were they chosen?

See the [using QSMxT](/using-qsmxt) page for the underlying algorithms used for each step.

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

