from nipype import JoinNode, Node, Workflow
from nipype.interfaces.utility import Function, IdentityInterface
from IPython.display import Image

def get_data_from_id(id):
    """Generate a random number based on id"""
    import numpy as np
    return id + np.random.rand()

def merge_and_scale_data(data2):
    """Scale the input list by 1000"""
    import numpy as np
    return (np.array(data2) * 1000).tolist()


node1 = Node(Function(input_names=['id'],
                      output_names=['data1'],
                      function=get_data_from_id),
             name='get_data')
node1.iterables = ('id', [1, 2, 3])

node2 = JoinNode(Function(input_names=['data2'],
                          output_names=['data_scaled'],
                          function=merge_and_scale_data),
                 name='scale_data',
                 joinsource=node1,
                 joinfield=['data2'])

wf = Workflow(name='testjoin')
wf.connect(node1, 'data1', node2, 'data2')
eg = wf.run('MultiProc', plugin_args={'n_procs': 9})

wf.write_graph(graph2use='exec')
Image(filename='graph_detailed.png')