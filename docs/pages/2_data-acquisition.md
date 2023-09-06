---
layout: default
title: Data acquisition
permalink: /data-acquisition
nav_order: 2
---

<head>
  <link rel="stylesheet" href="https://maxcdn.bootstrapcdn.com/bootstrap/3.4.1/css/bootstrap.min.css">
  <script src="https://ajax.googleapis.com/ajax/libs/jquery/3.6.0/jquery.min.js"></script>
  <script src="https://maxcdn.bootstrapcdn.com/bootstrap/3.4.1/js/bootstrap.min.js"></script>
</head>

# Acquiring data for QSM

Data acquisition for QSM largely depends on your application, imaging goals, and constraints. This page provides some general guidelines.

**Acquisition type**: QSM reconstruction requires a T2\*-weighted acquisition such as 3D-GRE. The T2\* weighting is important because it provides sensitivity to susceptibility variations, which is necessary to derive magnetic susceptibility. Fast imaging techniques such as 3D-EPI and others are sometimes employed.

**Coil combination**: A complex-domain coil combination method must be used for QSM, rather than the popular *sum of squares* technique. While *sum of squares* works well for magnitude images and may be used with some success for Susceptibility-Weighted Imaging (SWI), it can result in phase singularities that lead to an unresolvable magnetic field and *wormhole artefacts* that render QSM results unusable.

**Spatial resolution**: Most QSM algorithms work best with isotropic resolutions. ~1mm^3 is a fairly typical resolution for QSM, though there is an arguable balance to strike for any given application.

**Single/multi-echo**: Shorter echo times improve estimation of strong susceptibility sources, while longer echo times improve estimation of more subtle susceptibility sources. Therefore, multi-echo sequences are recommended for QSM, because they provide a good balance of cross-tissue phase contrast and susceptibility estimation. The echo time that maximises phase CNR for a particular tissue is the T2* time of the tissue. Therefore, echo times that go well beyond the typical T2* times of the imaged object or have very low SNR are less likely to provide tangible benefits. Single-echo acquisitions are not usually recommended for QSM, though in practice are regularly used depending on imaging constraints.

**Flow compensation**: Flow compensation is often recommended for QSM and may improve field mapping and susceptibility estimation. However, recent investigations indicate that the effects of flow compensation may be insignificant for most QSM applications. In practice, it is also difficult to use flow compensation across multiple echoes using standard product sequences.

<script>
$(document).ready(function(){
    $('[data-toggle="popover"]').popover();   
});
$("[data-toggle=popover]")
.popover({html:true})
</script>

